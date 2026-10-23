use super::*;

use nara_tooling::{
    EditorDocumentId, EditorDocumentKind, EditorPersistenceRejection,
    EditorPersistenceRequestResult, PrefabCompositionCommand, SceneCompositionCommand,
};

#[derive(Clone, Copy)]
enum ClosingMutation {
    RemoveSelectedEntity,
    Undo,
    Redo,
}

#[test]
fn closing_session_rejects_new_runtime_edits_while_still_allowing_stop() {
    use nara_tooling::{EditorRuntimeEditRejection, EditorRuntimeEditRequest};

    let (_project, mut editor) = running_editor_for_close("closing-runtime-edit");
    let frame = editor.current_frame();
    let request = EditorRuntimeEditRequest {
        generation: frame.play().generation().unwrap(),
        document_revision: frame.play().current_revision().unwrap(),
        entity: nara_identity::SceneEntityId::new("player").unwrap(),
        component: ComponentTypeId::new("nara.sprite.Sprite"),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("layer"),
        value: ComponentValue::I64(4),
    };
    let _ = editor.advance_close();
    let submitted = editor.request_runtime_edit_request(request);
    stop_and_close(&mut editor);
    assert_eq!(submitted, Err(EditorRuntimeEditRejection::Busy));
}

#[test]
fn closing_session_rejects_new_apply_changes_while_still_allowing_stop() {
    use nara_tooling::{EditorApplyChangesRejection, SceneApplyChangesRequest};

    let (_project, mut editor) = running_editor_for_close("closing-apply-changes");
    let _ = editor.advance_close();
    let submitted = editor.request_apply_changes(SceneApplyChangesRequest::new(
        nara_identity::SceneEntityId::new("player").unwrap(),
        [ComponentTypeId::new("nara.sprite.Sprite")],
    ));
    stop_and_close(&mut editor);
    assert_eq!(submitted, Err(EditorApplyChangesRejection::Busy));
}

#[test]
fn closing_session_rejects_previously_queued_apply_changes_before_source_publication() {
    use nara_tooling::{
        EditorApplyChangesRejection, EditorApplyChangesResult, EditorRuntimeEditRequest,
        EditorRuntimeEditResult, SceneApplyChangesRequest,
    };

    let (_project, mut editor) = running_editor_for_close("closing-queued-apply-changes");
    let frame = editor.current_frame();
    let document = frame.workspace().active_document.unwrap();
    editor
        .request_runtime_edit_request(EditorRuntimeEditRequest {
            generation: frame.play().generation().unwrap(),
            document_revision: frame.play().current_revision().unwrap(),
            entity: nara_identity::SceneEntityId::new("player").unwrap(),
            component: ComponentTypeId::new("nara.sprite.Sprite"),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("layer"),
            value: ComponentValue::I64(4),
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while matches!(
        editor.runtime_edit_result(),
        Some(EditorRuntimeEditResult::Pending(_))
    ) {
        assert!(
            Instant::now() < deadline,
            "the valid runtime edit did not settle"
        );
        editor.drive_editor_frame(Duration::ZERO);
    }
    assert!(matches!(
        editor.runtime_edit_result(),
        Some(EditorRuntimeEditResult::Applied(_))
    ));
    assert!(editor.acknowledge_runtime_edit_result());
    let before = document_state(&editor, document);
    editor
        .request_apply_changes(SceneApplyChangesRequest::new(
            nara_identity::SceneEntityId::new("player").unwrap(),
            [ComponentTypeId::new("nara.sprite.Sprite")],
        ))
        .unwrap();
    let _ = editor.advance_close();
    editor.drive_editor_frame(Duration::ZERO);
    let after = document_state(&editor, document);
    let result = editor.apply_changes_result().cloned();
    stop_and_close(&mut editor);
    assert_eq!(
        after, before,
        "queued Apply Changes must not publish through a closed workspace"
    );
    assert!(
        matches!(
            result,
            Some(EditorApplyChangesResult::Rejected {
                reason: EditorApplyChangesRejection::Busy,
                ..
            })
        ),
        "closing must return the owning refusal, not silently drop the captured request: {result:?}"
    );
}

fn running_editor_for_close(label: &str) -> (TestProject, editor::EditorProjectSession) {
    use nara_tooling::{EditorPlayCommand, EditorPlayRequestResult, EditorPlayState};

    let project = TestProject::with_image(label);
    let mut editor = editor::EditorProjectSession::open_trusted_compiled(
        project.writable_capability(),
        editor::TrustedCompiledEditorProjectIntent::new(),
    )
    .unwrap();
    assert_eq!(
        editor.request_play(EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while editor.play_view().state() != EditorPlayState::Running {
        assert!(
            Instant::now() < deadline,
            "the closing fixture Play did not start"
        );
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    (project, editor)
}

fn stop_and_close(editor: &mut editor::EditorProjectSession) {
    use nara_tooling::{EditorPlayCommand, EditorPlayRequestResult, EditorPlayState};

    assert_eq!(
        editor.request_play(EditorPlayCommand::Stop),
        EditorPlayRequestResult::Accepted
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while editor.play_view().state() != EditorPlayState::Empty {
        assert!(
            Instant::now() < deadline,
            "closing must retain Stop admission"
        );
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    let _ = drive_editor_close_to_completion(editor);
}

#[test]
fn closing_session_fences_the_common_workspace_command_ingress() {
    let project = TestProject::new("closing-workspace-ingress");
    fs::write(
        project.root.join("scenes/startup.scene.json"),
        editable_authoring_scene("original")
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let mut editor = open_generic_editor_with_ready_index(&project);
    let document = editor.workspace().active_document().unwrap();
    let base_revision = editor.workspace().document_revision(document).unwrap();
    let before = document_state(&editor, document);
    let _ = editor.advance_close();
    let report = editor.apply_workspace_command(EditorWorkspaceCommand::ApplyScenePatch {
        document,
        base_revision,
        patch: ScenePatchDocument::new([ScenePatchOperation::AddEntity {
            entity: SceneEntityRecord::new(nara_identity::SceneEntityId::new("late").unwrap()),
            placement: nara_scene::ScenePlacement::last(nara_scene::ScenePlacementOwner::Root),
        }]),
    });
    let after = document_state(&editor, document);
    let _ = drive_editor_close_to_completion(&mut editor);
    assert!(
        !report.applied,
        "the common ingress admitted work after close"
    );
    assert_eq!(after, before);
}

#[cfg(feature = "runtime-ui")]
#[test]
fn closing_session_rejects_gesture_preview_and_commit_without_consuming_its_draft() {
    use nara_ui::__private::UiAuthoringDirectEdit;

    let project = TestProject::new("closing-gesture-commit");
    project.configure_authoring_ui("Closing Gesture Commit");
    project.write_ui_authoring_documents(None, None);
    let mut editor = editor::EditorProjectSession::open_trusted_compiled(
        project.writable_capability(),
        editor::TrustedCompiledEditorProjectIntent::new(),
    )
    .unwrap();
    let scene = editor.workspace().active_document().unwrap();
    let opened = open_prefab_from_scene(
        &mut editor,
        scene,
        &nara_identity::SceneEntityId::new("ui-anchor").unwrap(),
    );
    let document = opened.opened_document.unwrap();
    let entity = nara_identity::SceneEntityId::new("image-node").unwrap();
    let frame = editor.drive_editor_frame(Duration::ZERO);
    let baseline = editor
        .current_ui_authoring_frame()
        .unwrap()
        .lower_direct_edit(&UiAuthoringDirectEdit::ResizeBy {
            entity: entity.clone(),
            delta: [0.0, 0.0],
        })
        .unwrap();
    let began = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [
                EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                    document,
                    entity: Some(entity.clone()),
                }),
                EditorAction::Gesture(EditorGestureAction::Begin { baseline }),
            ],
        ))
        .unwrap();
    let EditorActionOutcome::Gesture(EditorGestureOutcome::Began(gesture)) = &began.outcomes()[1]
    else {
        panic!("the live Session must admit the gesture baseline: {began:?}");
    };
    let gesture = gesture.stamp().id();
    let frame = editor.drive_editor_frame(Duration::ZERO);
    let edit = editor
        .current_ui_authoring_frame()
        .unwrap()
        .lower_direct_edit(&UiAuthoringDirectEdit::ResizeBy {
            entity,
            delta: [10.0, 0.0],
        })
        .unwrap();
    let before = document_state(&editor, document);
    let _ = editor.advance_close();
    let preview = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Gesture(EditorGestureAction::Preview {
                gesture,
                edit: edit.clone(),
            })],
        ))
        .unwrap();
    let commit = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Gesture(EditorGestureAction::Commit {
                gesture,
                edit,
            })],
        ))
        .unwrap();
    let after = document_state(&editor, document);
    let cancel = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Gesture(EditorGestureAction::Cancel {
                gesture,
                reason: EditorGestureCancelReason::TargetRetired,
            })],
        ))
        .unwrap();
    let _ = drive_editor_close_to_completion(&mut editor);
    assert_eq!(
        after, before,
        "a late gesture cannot publish after physical close"
    );
    for report in [preview, commit] {
        assert!(
            matches!(&report.outcomes()[0], EditorActionOutcome::Workspace(report) if !report.applied)
        );
    }
    assert!(
        matches!(
            &cancel.outcomes()[0],
            EditorActionOutcome::Gesture(EditorGestureOutcome::Cancelled(
                EditorGestureCancelReason::TargetRetired
            ))
        ),
        "rejected closing work must not consume the draft before explicit cancellation: {cancel:?}"
    );
}

#[test]
fn closing_scene_rejects_ordinary_mutation_without_changing_document_state() {
    assert_closing_session_rejects_mutation(
        EditorDocumentKind::Scene,
        ClosingMutation::RemoveSelectedEntity,
    );
}

#[test]
fn closing_prefab_rejects_ordinary_mutation_without_changing_document_state() {
    assert_closing_session_rejects_mutation(
        EditorDocumentKind::Prefab,
        ClosingMutation::RemoveSelectedEntity,
    );
}

#[test]
fn closing_scene_rejects_undo_without_changing_document_state() {
    assert_closing_session_rejects_mutation(EditorDocumentKind::Scene, ClosingMutation::Undo);
}

#[test]
fn closing_prefab_rejects_undo_without_changing_document_state() {
    assert_closing_session_rejects_mutation(EditorDocumentKind::Prefab, ClosingMutation::Undo);
}

#[test]
fn closing_scene_rejects_redo_without_changing_document_state() {
    assert_closing_session_rejects_mutation(EditorDocumentKind::Scene, ClosingMutation::Redo);
}

#[test]
fn closing_prefab_rejects_redo_without_changing_document_state() {
    assert_closing_session_rejects_mutation(EditorDocumentKind::Prefab, ClosingMutation::Redo);
}

fn assert_closing_session_rejects_mutation(kind: EditorDocumentKind, mutation: ClosingMutation) {
    let project = TestProject::new("closing-session-mutation-admission");
    fs::write(
        project.root.join("scenes/startup.scene.json"),
        editable_authoring_scene("original")
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    fs::write(
        project.root.join("prefabs/ordinary.prefab.json"),
        editable_authoring_prefab("original")
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let mut editor = open_generic_editor_with_ready_index(&project);
    let (document, path) = match kind {
        EditorDocumentKind::Scene => (
            editor.workspace().active_document().unwrap(),
            project.root.join("scenes/startup.scene.json"),
        ),
        EditorDocumentKind::Prefab => {
            let index = editor.project_source_index_snapshot().unwrap();
            let document = opened_document(editor.open_indexed_authoring_document(
                index,
                "prefabs/ordinary.prefab.json",
                crate::project_host::authoring::AuthoringDocumentKind::Prefab,
            ));
            (document, project.root.join("prefabs/ordinary.prefab.json"))
        }
        EditorDocumentKind::Extension(_) => unreachable!("only Scene and Prefab are tested"),
    };
    let persisted = fs::read(&path).unwrap();
    make_authoring_name_dirty(&mut editor, document, "unsaved");
    if matches!(mutation, ClosingMutation::Redo) {
        let base_revision = editor.workspace().document_revision(document).unwrap();
        let frame = editor.current_frame();
        let report = editor
            .dispatch_editor_actions(EditorActionBatch::new(
                frame.revision(),
                [EditorAction::Mutate(EditorDocumentMutation::Undo {
                    document,
                    base_revision,
                })],
            ))
            .unwrap();
        assert!(matches!(
            &report.outcomes()[0],
            EditorActionOutcome::Workspace(report) if report.applied
        ));
    }
    let entity = nara_identity::SceneEntityId::new("editable-name").unwrap();
    let frame = editor.drive_editor_frame(Duration::ZERO);
    let selected = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Navigate(
                EditorNavigationAction::SelectEntity {
                    document,
                    entity: Some(entity.clone()),
                },
            )],
        ))
        .unwrap();
    assert!(matches!(
        &selected.outcomes()[0],
        EditorActionOutcome::Workspace(report) if report.applied
    ));
    let history = if let Some(slot) = editor.workspace().scene(document) {
        slot.session().history_status()
    } else {
        editor
            .workspace()
            .prefab(document)
            .unwrap()
            .session()
            .history_status()
    };
    assert_eq!(
        (history.undo_depth, history.redo_depth),
        if matches!(mutation, ClosingMutation::Redo) {
            (0, 1)
        } else {
            (1, 0)
        }
    );
    let before = document_state(&editor, document);
    let active_before = editor.workspace().active_document();
    let base_revision = editor.workspace().document_revision(document).unwrap();
    let action = match mutation {
        ClosingMutation::RemoveSelectedEntity => match kind {
            EditorDocumentKind::Scene => EditorDocumentMutation::ComposeScene {
                document,
                base_revision,
                command: SceneCompositionCommand::RemoveEntity { entity },
            },
            EditorDocumentKind::Prefab => EditorDocumentMutation::ComposePrefab {
                document,
                base_revision,
                command: PrefabCompositionCommand::RemoveEntity { entity },
            },
            EditorDocumentKind::Extension(_) => unreachable!("only Scene and Prefab are tested"),
        },
        ClosingMutation::Undo => EditorDocumentMutation::Undo {
            document,
            base_revision,
        },
        ClosingMutation::Redo => EditorDocumentMutation::Redo {
            document,
            base_revision,
        },
    };

    let _ = editor.advance_close();
    let save = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Persistence(EditorPersistenceAction::Save {
                document,
                base_revision,
            })],
        ))
        .unwrap();
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Mutate(action)],
        ))
        .unwrap();
    let after = document_state(&editor, document);
    let active_after = editor.workspace().active_document();
    let _ = drive_editor_close_to_completion(&mut editor);

    assert_eq!(
        save.outcomes(),
        &[EditorActionOutcome::Persistence(
            EditorPersistenceRequestResult::Rejected(EditorPersistenceRejection::Busy)
        )]
    );
    assert_eq!(fs::read(&path).unwrap(), persisted);
    assert_eq!(active_after, active_before);
    assert_eq!(
        after, before,
        "a closing Session must preserve document, revision, content state, history, selection and saved state"
    );
    assert!(
        matches!(
            &report.outcomes()[0],
            EditorActionOutcome::Workspace(report) if !report.applied
        ),
        "a closing Session must reject ordinary mutations: {report:?}"
    );
}

fn document_state(
    editor: &editor::EditorProjectSession,
    document: EditorDocumentId,
) -> impl fmt::Debug + PartialEq + use<> {
    if let Some(slot) = editor.workspace().scene(document) {
        (
            slot.session().document().to_json_string().unwrap(),
            slot.revision(),
            slot.session().content_state(),
            slot.session().history_status(),
            slot.selection().clone(),
            slot.is_dirty(),
            slot.saved_revision(),
            slot.saved_content_state(),
            slot.saved_digest(),
        )
    } else {
        let slot = editor.workspace().prefab(document).unwrap();
        (
            slot.session().document().to_json_string().unwrap(),
            slot.revision(),
            slot.session().content_state(),
            slot.session().history_status(),
            slot.selection().clone(),
            slot.is_dirty(),
            slot.saved_revision(),
            slot.saved_content_state(),
            slot.saved_digest(),
        )
    }
}
