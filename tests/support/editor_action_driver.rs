#![allow(dead_code)]

use nara::{
    project_host::EditorProjectSession,
    reflect::{ComponentFieldId, ComponentSchemaVersion, ComponentTypeId, ComponentValue},
    scene::SceneEntityId,
    tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorActionRejection,
        EditorApplyChangesRejection, EditorCloseDecision, EditorDocumentMutation,
        EditorNavigationAction, EditorPersistenceAction, EditorPersistenceCommand,
        EditorPersistenceRequestResult, EditorPlayAction, EditorPlayCommand,
        EditorPlayRequestResult, EditorRuntimeEditRejection, EditorRuntimeEditRequest,
        EditorWorkspaceCommand, EditorWorkspaceCommandReport, EditorWorkspaceIntent,
        EditorWorkspaceIntentAction, EditorWorkspaceIntentRequestResult, SceneApplyChangesRequest,
    },
};

pub fn dispatch(
    editor: &mut EditorProjectSession,
    actions: impl IntoIterator<Item = EditorAction>,
) -> Vec<EditorActionOutcome> {
    let frame = editor.current_frame();
    editor
        .dispatch_editor_actions(EditorActionBatch::new(frame.revision(), actions))
        .expect("the test driver dispatches each action list from the current Editor frame")
        .into_outcomes()
}

pub fn dispatch_one(
    editor: &mut EditorProjectSession,
    action: EditorAction,
) -> EditorActionOutcome {
    let outcomes = dispatch(editor, [action]);
    assert_eq!(outcomes.len(), 1);
    outcomes.into_iter().next().unwrap()
}

pub fn apply_workspace_command(
    editor: &mut EditorProjectSession,
    command: EditorWorkspaceCommand,
) -> EditorWorkspaceCommandReport {
    let action = match command {
        EditorWorkspaceCommand::SetActiveDocument { document } => {
            EditorAction::Navigate(EditorNavigationAction::SetActiveDocument { document })
        }
        EditorWorkspaceCommand::SelectEntity { document, entity } => {
            EditorAction::Navigate(EditorNavigationAction::SelectEntity { document, entity })
        }
        EditorWorkspaceCommand::MarkExternalChanged { document } => {
            EditorAction::Navigate(EditorNavigationAction::MarkExternalChanged { document })
        }
        EditorWorkspaceCommand::ApplyInspectorCommand {
            document,
            base_revision,
            command,
        } => EditorAction::Mutate(EditorDocumentMutation::ApplyInspectorCommand {
            document,
            base_revision,
            command,
        }),
        EditorWorkspaceCommand::ApplyScenePatch {
            document,
            base_revision,
            patch,
        } => EditorAction::Mutate(EditorDocumentMutation::ApplyScenePatch {
            document,
            base_revision,
            patch,
        }),
        EditorWorkspaceCommand::ApplyPrefabPatch {
            document,
            base_revision,
            patch,
        } => EditorAction::Mutate(EditorDocumentMutation::ApplyPrefabPatch {
            document,
            base_revision,
            patch,
        }),
        EditorWorkspaceCommand::ComposeScene {
            document,
            base_revision,
            command,
        } => EditorAction::Mutate(EditorDocumentMutation::ComposeScene {
            document,
            base_revision,
            command,
        }),
        EditorWorkspaceCommand::Undo {
            document,
            base_revision,
        } => EditorAction::Mutate(EditorDocumentMutation::Undo {
            document,
            base_revision,
        }),
        EditorWorkspaceCommand::Redo {
            document,
            base_revision,
        } => EditorAction::Mutate(EditorDocumentMutation::Redo {
            document,
            base_revision,
        }),
        EditorWorkspaceCommand::CloseDocument { .. } => {
            panic!("close must use the explicit workspace-intent action")
        }
    };
    match dispatch_one(editor, action) {
        EditorActionOutcome::Workspace(report) => report,
        outcome => panic!("expected a workspace outcome, got {outcome:?}"),
    }
}

pub fn request_play(
    editor: &mut EditorProjectSession,
    command: EditorPlayCommand,
) -> EditorPlayRequestResult {
    let frame = editor.current_frame();
    let action = match command {
        EditorPlayCommand::Play => {
            let scene = frame
                .workspace()
                .active_scene
                .as_ref()
                .expect("the test must dispatch Play from a frame with an active document");
            EditorPlayAction::Play {
                document: scene.document,
                base_revision: scene.revision,
            }
        }
        EditorPlayCommand::Cancel => EditorPlayAction::Cancel,
        EditorPlayCommand::Pause => EditorPlayAction::Pause,
        EditorPlayCommand::Resume => EditorPlayAction::Resume,
        EditorPlayCommand::StepFixedTick => EditorPlayAction::StepFixedTick,
        EditorPlayCommand::Stop => EditorPlayAction::Stop,
        EditorPlayCommand::Restart => EditorPlayAction::Restart,
        EditorPlayCommand::RetryRetirement => EditorPlayAction::RetryRetirement,
        EditorPlayCommand::RetryClose => EditorPlayAction::RetryClose,
        EditorPlayCommand::AcknowledgeResult => EditorPlayAction::AcknowledgeResult,
    };
    match dispatch_one(editor, EditorAction::Play(action)) {
        EditorActionOutcome::Play(result) => result,
        outcome => panic!("expected a Play outcome, got {outcome:?}"),
    }
}

pub fn request_persistence(
    editor: &mut EditorProjectSession,
    command: EditorPersistenceCommand,
) -> EditorPersistenceRequestResult {
    let frame = editor.current_frame();
    let action = match command {
        EditorPersistenceCommand::Save { document } => {
            let document = document
                .or(frame.workspace().active_document)
                .expect("the test must dispatch Save for an open document");
            let base_revision = frame
                .workspace()
                .document(document)
                .expect("the test must dispatch Save for an open document")
                .revision;
            EditorPersistenceAction::Save {
                document,
                base_revision,
            }
        }
        EditorPersistenceCommand::Reopen { document } => {
            let target = frame.reopen_target();
            if let Some(document) = document {
                let target_document = match target {
                    nara::tooling::EditorReopenTarget::OpenDocument { document, .. }
                    | nara::tooling::EditorReopenTarget::ClosedSource { document } => document,
                };
                assert_eq!(target_document, document);
            }
            EditorPersistenceAction::Reopen { target }
        }
        EditorPersistenceCommand::AcknowledgeResult => EditorPersistenceAction::AcknowledgeResult,
    };
    match dispatch_one(editor, EditorAction::Persistence(action)) {
        EditorActionOutcome::Persistence(result) => result,
        outcome => panic!("expected a persistence outcome, got {outcome:?}"),
    }
}

pub fn request_workspace_intent(
    editor: &mut EditorProjectSession,
    intent: EditorWorkspaceIntent,
) -> EditorWorkspaceIntentRequestResult {
    match dispatch_one(
        editor,
        EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::Request(intent)),
    ) {
        EditorActionOutcome::WorkspaceIntent(result) => result,
        outcome => panic!("expected a workspace-intent outcome, got {outcome:?}"),
    }
}

pub fn resolve_workspace_intent(
    editor: &mut EditorProjectSession,
    decision: EditorCloseDecision,
) -> EditorWorkspaceIntentRequestResult {
    match dispatch_one(
        editor,
        EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::Resolve(decision)),
    ) {
        EditorActionOutcome::WorkspaceIntent(result) => result,
        outcome => panic!("expected a workspace-intent outcome, got {outcome:?}"),
    }
}

pub fn acknowledge_workspace_intent_result(editor: &mut EditorProjectSession) -> bool {
    match dispatch_one(
        editor,
        EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::AcknowledgeResult),
    ) {
        EditorActionOutcome::Acknowledged(
            nara::tooling::EditorActionAcknowledgement::WorkspaceIntent(acknowledged),
        ) => acknowledged,
        outcome => panic!("expected an acknowledgement outcome, got {outcome:?}"),
    }
}

pub fn request_runtime_edit(
    editor: &mut EditorProjectSession,
    entity: SceneEntityId,
    component: ComponentTypeId,
    component_version: ComponentSchemaVersion,
    field: ComponentFieldId,
    value: ComponentValue,
) -> Result<(), EditorRuntimeEditRejection> {
    let frame = editor.current_frame();
    let scene = frame
        .workspace()
        .active_scene
        .as_ref()
        .expect("the test must dispatch a runtime edit for an open document");
    let request = EditorRuntimeEditRequest {
        generation: frame
            .play()
            .generation()
            .expect("the test must dispatch a runtime edit while Play is active"),
        document_revision: scene.revision,
        entity,
        component,
        component_version,
        field,
        value,
    };
    request_runtime_edit_request(editor, request)
}

pub fn request_runtime_edit_request(
    editor: &mut EditorProjectSession,
    request: EditorRuntimeEditRequest,
) -> Result<(), EditorRuntimeEditRejection> {
    let frame = editor.current_frame();
    let document = frame
        .workspace()
        .active_scene
        .as_ref()
        .map(|scene| scene.document)
        .expect("the test must dispatch a runtime edit for an open document");
    match dispatch_one(editor, EditorAction::RuntimeEdit { document, request }) {
        EditorActionOutcome::RuntimeEdit(result) => result,
        EditorActionOutcome::Rejected(EditorActionRejection::RuntimeGenerationChanged {
            ..
        }) => Err(EditorRuntimeEditRejection::StaleGeneration),
        EditorActionOutcome::Rejected(
            EditorActionRejection::StaleDocumentFact { .. }
            | EditorActionRejection::RuntimeDocumentChanged { .. },
        ) => Err(EditorRuntimeEditRejection::StaleDocument),
        outcome => panic!("expected a runtime-edit outcome, got {outcome:?}"),
    }
}

pub fn acknowledge_runtime_edit_result(editor: &mut EditorProjectSession) -> bool {
    match dispatch_one(editor, EditorAction::AcknowledgeRuntimeEditResult) {
        EditorActionOutcome::Acknowledged(
            nara::tooling::EditorActionAcknowledgement::RuntimeEdit(acknowledged),
        ) => acknowledged,
        outcome => panic!("expected an acknowledgement outcome, got {outcome:?}"),
    }
}

pub fn request_apply_changes(
    editor: &mut EditorProjectSession,
    request: SceneApplyChangesRequest,
) -> Result<(), EditorApplyChangesRejection> {
    let frame = editor.current_frame();
    let scene = frame
        .workspace()
        .active_scene
        .as_ref()
        .expect("the test must dispatch Apply Changes for an open document");
    let generation = frame
        .play()
        .generation()
        .expect("the test must dispatch Apply Changes while Play is active");
    match dispatch_one(
        editor,
        EditorAction::ApplyChanges {
            document: scene.document,
            base_revision: scene.revision,
            generation,
            request,
        },
    ) {
        EditorActionOutcome::ApplyChanges(result) => result,
        EditorActionOutcome::Rejected(EditorActionRejection::RuntimeGenerationChanged {
            ..
        }) => Err(EditorApplyChangesRejection::InvalidRuntimeState),
        EditorActionOutcome::Rejected(
            EditorActionRejection::StaleDocumentFact { .. }
            | EditorActionRejection::RuntimeDocumentChanged { .. },
        ) => Err(EditorApplyChangesRejection::StaleDocument),
        outcome => panic!("expected an Apply Changes outcome, got {outcome:?}"),
    }
}

pub fn acknowledge_apply_changes_result(editor: &mut EditorProjectSession) -> bool {
    match dispatch_one(editor, EditorAction::AcknowledgeApplyChangesResult) {
        EditorActionOutcome::Acknowledged(
            nara::tooling::EditorActionAcknowledgement::ApplyChanges(acknowledged),
        ) => acknowledged,
        outcome => panic!("expected an acknowledgement outcome, got {outcome:?}"),
    }
}
