use std::{collections::BTreeSet, sync::Arc};

use nara_diagnostic::{
    Diagnostic, DiagnosticCode, DiagnosticField, DiagnosticFieldKey, SafeSummary,
};
use nara_scene::SceneAuthoringRevision;
use nara_tooling::{
    EditorAction, EditorActionAcknowledgement, EditorActionBatch, EditorActionBatchReport,
    EditorActionOutcome, EditorActionRejection, EditorAtomicFieldEdit, EditorDocumentId,
    EditorDocumentMutation, EditorFieldFeedback, EditorFieldFeedbackReason, EditorFieldTarget,
    EditorFieldValidationFailure, EditorFrameModel, EditorGestureAction, EditorGestureCancelReason,
    EditorGestureOutcome, EditorGestureRejection, EditorNavigationAction, EditorPersistenceAction,
    EditorPersistenceCommand, EditorPlayAction, EditorPlayCommand, EditorReopenTarget,
    EditorWorkspaceCommand, EditorWorkspaceIntent, EditorWorkspaceIntentAction,
};

use crate::project_host::authoring::ProjectSourceIndexSnapshot;

use super::{EditorProjectSession, EditorRuntimeOwnerState};

#[derive(Clone, Copy)]
enum EditorFieldCommitMode {
    Discrete,
    Continuous,
}

impl EditorProjectSession {
    /// Applies one ordered action list produced from the current immutable Editor frame.
    ///
    /// A frame accepts at most one batch. Every revision-bound action is checked against both
    /// its originating frame and the current document/runtime authority before any operation is
    /// requested. A second document mutation from the same frame is rejected rather than rebased.
    pub fn dispatch_editor_actions(
        &mut self,
        batch: EditorActionBatch,
    ) -> Result<EditorActionBatchReport, EditorActionRejection> {
        let batch_field_target = batch.first_field_target();
        let received = batch.frame_revision();
        let frame = match self.published_frame.as_mut() {
            None => Err(EditorActionRejection::NoPublishedFrame),
            Some(published) => {
                let expected = published.model.revision();
                if received != expected {
                    Err(EditorActionRejection::StaleFrame { expected, received })
                } else if published.dispatched {
                    Err(EditorActionRejection::FrameAlreadyDispatched { revision: expected })
                } else {
                    published.dispatched = true;
                    Ok((
                        Arc::clone(&published.model),
                        published.source_index.as_ref().map(Arc::clone),
                    ))
                }
            }
        };
        let (frame, source_index) = match frame {
            Ok(frame) => frame,
            Err(rejection) => {
                self.record_editor_action_rejection(rejection);
                if let Some(target) = batch_field_target {
                    self.field_feedback = Some(EditorFieldFeedback::new(
                        target,
                        EditorFieldFeedbackReason::StaleAction,
                    ));
                }
                return Err(rejection);
            }
        };
        let expected = frame.revision();

        let mut mutation_documents = BTreeSet::new();
        let outcomes = batch.into_actions().into_iter().map(|action| {
            let field_target = action.field_target();
            let outcome = if let Some(document) = action.revision_advancing_document()
                && !mutation_documents.insert(document)
            {
                EditorActionOutcome::Rejected(EditorActionRejection::MutationAlreadySubmitted {
                    document,
                })
            } else {
                self.dispatch_editor_action(&frame, source_index.as_ref(), action)
            };
            if let EditorActionOutcome::Rejected(rejection) = &outcome {
                self.record_editor_action_rejection(*rejection);
            }
            self.update_editor_field_feedback(field_target, &outcome);
            outcome
        });
        Ok(EditorActionBatchReport::new(expected, outcomes))
    }

    fn record_editor_action_rejection(&mut self, rejection: EditorActionRejection) {
        self.diagnostics
            .push(editor_action_rejection_diagnostic(rejection));
    }

    fn dispatch_editor_action(
        &mut self,
        frame: &EditorFrameModel,
        source_index: Option<&Arc<ProjectSourceIndexSnapshot>>,
        action: EditorAction,
    ) -> EditorActionOutcome {
        match action {
            EditorAction::Navigate(action) => self.dispatch_navigation(frame, action),
            EditorAction::Mutate(action) => self.dispatch_document_mutation(frame, action),
            EditorAction::Persistence(action) => self.dispatch_persistence(frame, action),
            EditorAction::ProjectBrowser(action) => EditorActionOutcome::ProjectBrowser(
                self.dispatch_project_browser_action(frame, source_index, action),
            ),
            EditorAction::ProjectContent(action) => {
                EditorActionOutcome::ProjectContent(self.request_project_content_refresh(action))
            }
            EditorAction::WorkspaceIntent(action) => self.dispatch_workspace_intent(frame, action),
            EditorAction::Play(action) => self.dispatch_play(frame, action),
            EditorAction::ReportFieldValidation(failure) => {
                self.dispatch_field_validation(frame, failure)
            }
            EditorAction::Gesture(action) => self.dispatch_gesture(frame, action),
            EditorAction::RuntimeEdit { document, request } => {
                if let Err(rejection) =
                    self.verify_document_fact(frame, document, request.document_revision)
                {
                    return EditorActionOutcome::Rejected(rejection);
                }
                if let Err(rejection) =
                    self.verify_runtime_fact(frame, document, request.generation)
                {
                    return EditorActionOutcome::Rejected(rejection);
                }
                EditorActionOutcome::RuntimeEdit(self.request_runtime_edit_request(request))
            }
            EditorAction::ApplyChanges {
                document,
                base_revision,
                generation,
                request,
            } => {
                if let Err(rejection) = self.verify_document_fact(frame, document, base_revision) {
                    return EditorActionOutcome::Rejected(rejection);
                }
                if let Err(rejection) = self.verify_runtime_fact(frame, document, generation) {
                    return EditorActionOutcome::Rejected(rejection);
                }
                EditorActionOutcome::ApplyChanges(self.request_apply_changes(request))
            }
            EditorAction::AcknowledgeRuntimeEditResult => EditorActionOutcome::Acknowledged(
                EditorActionAcknowledgement::RuntimeEdit(self.acknowledge_runtime_edit_result()),
            ),
            EditorAction::AcknowledgeApplyChangesResult => EditorActionOutcome::Acknowledged(
                EditorActionAcknowledgement::ApplyChanges(self.acknowledge_apply_changes_result()),
            ),
        }
    }

    fn dispatch_navigation(
        &mut self,
        frame: &EditorFrameModel,
        action: EditorNavigationAction,
    ) -> EditorActionOutcome {
        if let EditorNavigationAction::OpenPrefabSource {
            document,
            base_revision,
            entity,
        } = action
        {
            if let Err(rejection) = self.verify_document_fact(frame, document, base_revision) {
                return EditorActionOutcome::Rejected(rejection);
            }
            let report = self.open_prefab_source(document, base_revision, &entity);
            let _ = self.diagnostics.extend(report.diagnostics.clone());
            if report.applied {
                #[cfg(feature = "runtime-ui")]
                self.reconcile_editor_gesture();
            }
            return EditorActionOutcome::Workspace(report);
        }
        let document = action.document();
        let Some(frame_document) = frame.workspace().document(document) else {
            return EditorActionOutcome::Rejected(EditorActionRejection::DocumentNotInFrame {
                document,
            });
        };
        if matches!(&action, EditorNavigationAction::SelectEntity { .. }) {
            let actual = self.workspace.active_document();
            if actual != Some(document) {
                return EditorActionOutcome::Rejected(
                    EditorActionRejection::ActiveDocumentChanged {
                        expected: document,
                        actual,
                    },
                );
            }
            if let Err(rejection) =
                self.verify_document_fact(frame, document, frame_document.revision)
            {
                return EditorActionOutcome::Rejected(rejection);
            }
        }
        let command = match action {
            EditorNavigationAction::OpenPrefabSource { .. } => unreachable!(),
            EditorNavigationAction::SetActiveDocument { document } => {
                EditorWorkspaceCommand::SetActiveDocument { document }
            }
            EditorNavigationAction::SelectEntity { document, entity } => {
                EditorWorkspaceCommand::SelectEntity { document, entity }
            }
            EditorNavigationAction::MarkExternalChanged { document } => {
                EditorWorkspaceCommand::MarkExternalChanged { document }
            }
        };
        let report = self.apply_workspace_command(command);
        if report.applied {
            #[cfg(feature = "runtime-ui")]
            self.reconcile_editor_gesture();
        }
        EditorActionOutcome::Workspace(report)
    }

    fn dispatch_field_validation(
        &mut self,
        frame: &EditorFrameModel,
        failure: EditorFieldValidationFailure,
    ) -> EditorActionOutcome {
        if let Err(rejection) = self.verify_document_fact(
            frame,
            failure.target().document(),
            failure.target().base_revision(),
        ) {
            return EditorActionOutcome::Rejected(rejection);
        }
        let _ = self
            .diagnostics
            .push(editor_field_validation_diagnostic(&failure));
        self.field_feedback = Some(EditorFieldFeedback::new(
            failure.target().clone(),
            EditorFieldFeedbackReason::InvalidValue(failure.expected()),
        ));
        EditorActionOutcome::Acknowledged(EditorActionAcknowledgement::FieldValidation)
    }

    fn update_editor_field_feedback(
        &mut self,
        target: Option<EditorFieldTarget>,
        outcome: &EditorActionOutcome,
    ) {
        let Some(target) = target else {
            return;
        };
        match outcome {
            EditorActionOutcome::Workspace(report) if report.applied => {
                if self
                    .field_feedback
                    .as_ref()
                    .is_some_and(|feedback| feedback.target().same_field(&target))
                {
                    self.field_feedback = None;
                }
            }
            EditorActionOutcome::Workspace(_) => {
                self.field_feedback = Some(EditorFieldFeedback::new(
                    target,
                    EditorFieldFeedbackReason::MutationRejected,
                ));
            }
            EditorActionOutcome::Rejected(_) => {
                self.field_feedback = Some(EditorFieldFeedback::new(
                    target,
                    EditorFieldFeedbackReason::StaleAction,
                ));
            }
            EditorActionOutcome::Persistence(_)
            | EditorActionOutcome::ProjectBrowser(_)
            | EditorActionOutcome::ProjectContent(_)
            | EditorActionOutcome::WorkspaceIntent(_)
            | EditorActionOutcome::Play(_)
            | EditorActionOutcome::Gesture(_)
            | EditorActionOutcome::RuntimeEdit(_)
            | EditorActionOutcome::ApplyChanges(_)
            | EditorActionOutcome::Acknowledged(_) => {}
        }
    }

    fn dispatch_document_mutation(
        &mut self,
        frame: &EditorFrameModel,
        action: EditorDocumentMutation,
    ) -> EditorActionOutcome {
        let document = action.document();
        let base_revision = action.base_revision();
        if let Err(rejection) = self.verify_document_fact(frame, document, base_revision) {
            return EditorActionOutcome::Rejected(rejection);
        }
        if let EditorDocumentMutation::ApplyPrefabFieldEdit { edit } = &action
            && !self.ui_authoring_field_edit_matches(frame, edit, EditorFieldCommitMode::Discrete)
        {
            return EditorActionOutcome::Rejected(EditorActionRejection::FieldContractMismatch {
                document,
            });
        }
        if let EditorDocumentMutation::ComposePrefab { command, .. } = &action
            && !self.ui_authoring_structure_matches(frame, document, base_revision, command)
        {
            return EditorActionOutcome::Rejected(
                EditorActionRejection::StructureContractMismatch { document },
            );
        }
        let publishes_prefab_structure =
            matches!(&action, EditorDocumentMutation::ComposePrefab { .. });
        let command = match action {
            EditorDocumentMutation::ApplyInspectorCommand {
                document,
                base_revision,
                command,
            } => EditorWorkspaceCommand::ApplyInspectorCommand {
                document,
                base_revision,
                command,
            },
            EditorDocumentMutation::ApplyScenePatch {
                document,
                base_revision,
                patch,
            } => EditorWorkspaceCommand::ApplyScenePatch {
                document,
                base_revision,
                patch,
            },
            EditorDocumentMutation::ApplyPrefabPatch {
                document,
                base_revision,
                patch,
            } => EditorWorkspaceCommand::ApplyPrefabPatch {
                document,
                base_revision,
                patch,
            },
            EditorDocumentMutation::ApplyAuthoringScalarEdit {
                document,
                base_revision,
                edit,
            } => EditorWorkspaceCommand::ApplyAuthoringScalarEdit {
                document,
                base_revision,
                edit,
            },
            EditorDocumentMutation::ApplyPrefabFieldEdit { edit } => {
                EditorWorkspaceCommand::ApplyPrefabPatch {
                    document: edit.target().document(),
                    base_revision: edit.target().base_revision(),
                    patch: edit.to_patch(),
                }
            }
            EditorDocumentMutation::ComposeScene {
                document,
                base_revision,
                command,
            } => EditorWorkspaceCommand::ComposeScene {
                document,
                base_revision,
                command,
            },
            EditorDocumentMutation::ComposePrefab {
                document,
                base_revision,
                command,
            } => EditorWorkspaceCommand::ComposePrefab {
                document,
                base_revision,
                command,
            },
            EditorDocumentMutation::Undo {
                document,
                base_revision,
            } => EditorWorkspaceCommand::Undo {
                document,
                base_revision,
            },
            EditorDocumentMutation::Redo {
                document,
                base_revision,
            } => EditorWorkspaceCommand::Redo {
                document,
                base_revision,
            },
        };
        let report = self.apply_workspace_command(command);
        if publishes_prefab_structure && report.applied {
            if self
                .gestures
                .active()
                .is_some_and(|gesture| gesture.baseline().target().document() == document)
            {
                let _ = self.cancel_editor_gesture(EditorGestureCancelReason::StaleRevision);
            }
            #[cfg(feature = "runtime-ui")]
            self.retire_ui_authoring_after_structure_publication(document);
        }
        let _ = self.diagnostics.extend(report.diagnostics.clone());
        EditorActionOutcome::Workspace(report)
    }

    fn dispatch_gesture(
        &mut self,
        frame: &EditorFrameModel,
        action: EditorGestureAction,
    ) -> EditorActionOutcome {
        match action {
            EditorGestureAction::Begin { baseline } => {
                let target = baseline.target();
                if self.workspace.active_document() != Some(target.document()) {
                    return EditorActionOutcome::Gesture(EditorGestureOutcome::Rejected(
                        EditorGestureRejection::BaselineMismatch,
                    ));
                }
                if let Err(rejection) =
                    self.verify_document_fact(frame, target.document(), target.base_revision())
                {
                    return EditorActionOutcome::Rejected(rejection);
                }
                let is_prefab =
                    frame
                        .workspace()
                        .document(target.document())
                        .is_some_and(|document| {
                            document.kind == nara_tooling::EditorDocumentKind::Prefab
                        });
                let Some(lane) = self.trusted_compiled() else {
                    return EditorActionOutcome::Gesture(EditorGestureOutcome::Rejected(
                        EditorGestureRejection::BaselineMismatch,
                    ));
                };
                if !is_prefab
                    || !self.workspace.__prefab_field_edit_matches(
                        lane.plan.schema_validation().registry(),
                        &baseline,
                    )
                    || !self.ui_authoring_field_edit_matches(
                        frame,
                        &baseline,
                        EditorFieldCommitMode::Continuous,
                    )
                {
                    return EditorActionOutcome::Gesture(EditorGestureOutcome::Rejected(
                        EditorGestureRejection::BaselineMismatch,
                    ));
                }
                let outcome = self.gestures.begin(baseline);
                if matches!(outcome, EditorGestureOutcome::Began(_)) {
                    self.last_gesture_cancellation = None;
                }
                EditorActionOutcome::Gesture(outcome)
            }
            EditorGestureAction::Preview { gesture, edit } => {
                let target = edit.target();
                if self.workspace.active_document() != Some(target.document()) {
                    return EditorActionOutcome::Gesture(
                        self.cancel_editor_gesture(EditorGestureCancelReason::SourceHandoff),
                    );
                }
                if let Err(rejection) =
                    self.verify_document_fact(frame, target.document(), target.base_revision())
                {
                    return EditorActionOutcome::Rejected(rejection);
                }
                if let Err(rejection) = self.gestures.validate_edit(gesture, &edit) {
                    return EditorActionOutcome::Gesture(EditorGestureOutcome::Rejected(rejection));
                }
                if !self.ui_authoring_field_edit_matches(
                    frame,
                    &edit,
                    EditorFieldCommitMode::Continuous,
                ) {
                    let reason = nara_tooling::EditorGestureCancelReason::InvalidPreview;
                    let outcome = self
                        .gestures
                        .cancel_active(reason)
                        .expect("a validated gesture remains active until cancellation");
                    self.last_gesture_cancellation = Some(reason);
                    return EditorActionOutcome::Gesture(outcome);
                }
                EditorActionOutcome::Gesture(self.gestures.preview(gesture, edit))
            }
            EditorGestureAction::Commit { gesture, edit } => {
                let target = edit.target().clone();
                if self.workspace.active_document() != Some(target.document()) {
                    return EditorActionOutcome::Gesture(
                        self.cancel_editor_gesture(EditorGestureCancelReason::SourceHandoff),
                    );
                }
                if let Err(rejection) =
                    self.verify_document_fact(frame, target.document(), target.base_revision())
                {
                    return EditorActionOutcome::Rejected(rejection);
                }
                if let Err(rejection) = self.gestures.validate_edit(gesture, &edit) {
                    return EditorActionOutcome::Gesture(EditorGestureOutcome::Rejected(rejection));
                }
                if !self.ui_authoring_field_edit_matches(
                    frame,
                    &edit,
                    EditorFieldCommitMode::Continuous,
                ) {
                    let reason = nara_tooling::EditorGestureCancelReason::InvalidPreview;
                    let outcome = self
                        .gestures
                        .cancel_active(reason)
                        .expect("a validated gesture remains active until cancellation");
                    self.last_gesture_cancellation = Some(reason);
                    return EditorActionOutcome::Gesture(outcome);
                }
                let edit = match self.gestures.commit(gesture, edit) {
                    Ok(edit) => edit,
                    Err(rejection) => {
                        return EditorActionOutcome::Gesture(EditorGestureOutcome::Rejected(
                            rejection,
                        ));
                    }
                };
                let report =
                    self.apply_workspace_command(EditorWorkspaceCommand::ApplyPrefabPatch {
                        document: target.document(),
                        base_revision: target.base_revision(),
                        patch: edit.to_patch(),
                    });
                let _ = self.diagnostics.extend(report.diagnostics.clone());
                self.last_gesture_cancellation = (!report.applied)
                    .then_some(nara_tooling::EditorGestureCancelReason::InvalidPreview);
                EditorActionOutcome::Workspace(report)
            }
            EditorGestureAction::Cancel { gesture, reason } => {
                let outcome = self.gestures.cancel(gesture, reason);
                if matches!(outcome, EditorGestureOutcome::Cancelled(_)) {
                    self.last_gesture_cancellation = Some(reason);
                }
                EditorActionOutcome::Gesture(outcome)
            }
        }
    }

    fn dispatch_persistence(
        &mut self,
        frame: &EditorFrameModel,
        action: EditorPersistenceAction,
    ) -> EditorActionOutcome {
        match action {
            EditorPersistenceAction::Save {
                document,
                base_revision,
            } => {
                if let Err(rejection) = self.verify_document_fact(frame, document, base_revision) {
                    return EditorActionOutcome::Rejected(rejection);
                }
                EditorActionOutcome::Persistence(self.request_persistence(
                    EditorPersistenceCommand::Save {
                        document: Some(document),
                    },
                ))
            }
            EditorPersistenceAction::Reopen { target } => {
                if target != frame.reopen_target() {
                    return EditorActionOutcome::Rejected(
                        EditorActionRejection::InvalidReopenTarget,
                    );
                }
                let document = match target {
                    EditorReopenTarget::OpenDocument {
                        document,
                        base_revision,
                    } => {
                        if let Err(rejection) =
                            self.verify_document_fact(frame, document, base_revision)
                        {
                            return EditorActionOutcome::Rejected(rejection);
                        }
                        document
                    }
                    EditorReopenTarget::Unavailable => {
                        return EditorActionOutcome::Rejected(
                            EditorActionRejection::InvalidReopenTarget,
                        );
                    }
                };
                let outcome = self.request_persistence(EditorPersistenceCommand::Reopen {
                    document: Some(document),
                });
                if outcome == nara_tooling::EditorPersistenceRequestResult::Accepted {
                    let _ = self.cancel_editor_gesture(EditorGestureCancelReason::Reopen);
                }
                EditorActionOutcome::Persistence(outcome)
            }
            EditorPersistenceAction::AcknowledgeResult => EditorActionOutcome::Persistence(
                self.request_persistence(EditorPersistenceCommand::AcknowledgeResult),
            ),
        }
    }

    fn dispatch_workspace_intent(
        &mut self,
        frame: &EditorFrameModel,
        action: EditorWorkspaceIntentAction,
    ) -> EditorActionOutcome {
        match action {
            EditorWorkspaceIntentAction::Request(intent) => {
                if let EditorWorkspaceIntent::CloseDocument { document } = intent
                    && frame.workspace().document(document).is_none()
                {
                    return EditorActionOutcome::Rejected(
                        EditorActionRejection::DocumentNotInFrame { document },
                    );
                }
                EditorActionOutcome::WorkspaceIntent(self.request_workspace_intent(intent))
            }
            EditorWorkspaceIntentAction::Resolve(decision) => {
                EditorActionOutcome::WorkspaceIntent(self.resolve_workspace_intent(decision))
            }
            EditorWorkspaceIntentAction::CommitPrepared => {
                EditorActionOutcome::WorkspaceIntent(self.commit_prepared_workspace_intent())
            }
            EditorWorkspaceIntentAction::AbortPrepared => {
                EditorActionOutcome::WorkspaceIntent(self.abort_prepared_workspace_intent())
            }
            EditorWorkspaceIntentAction::AcknowledgeResult => {
                EditorActionOutcome::Acknowledged(EditorActionAcknowledgement::WorkspaceIntent(
                    self.acknowledge_workspace_intent_result(),
                ))
            }
        }
    }

    fn dispatch_play(
        &mut self,
        frame: &EditorFrameModel,
        action: EditorPlayAction,
    ) -> EditorActionOutcome {
        let result = match action {
            EditorPlayAction::Play {
                document,
                base_revision,
            } => {
                if let Err(rejection) = self.verify_document_fact(frame, document, base_revision) {
                    return EditorActionOutcome::Rejected(rejection);
                }
                self.request_play_from_frame(document, base_revision)
            }
            EditorPlayAction::Cancel => self.request_play(EditorPlayCommand::Cancel),
            EditorPlayAction::Pause => self.request_play(EditorPlayCommand::Pause),
            EditorPlayAction::Resume => self.request_play(EditorPlayCommand::Resume),
            EditorPlayAction::StepFixedTick => self.request_play(EditorPlayCommand::StepFixedTick),
            EditorPlayAction::Stop => self.request_play(EditorPlayCommand::Stop),
            EditorPlayAction::Restart => self.request_play(EditorPlayCommand::Restart),
            EditorPlayAction::RetryRetirement => {
                self.request_play(EditorPlayCommand::RetryRetirement)
            }
            EditorPlayAction::RetryClose => self.request_play(EditorPlayCommand::RetryClose),
            EditorPlayAction::AcknowledgeResult => {
                self.request_play(EditorPlayCommand::AcknowledgeResult)
            }
        };
        EditorActionOutcome::Play(result)
    }

    fn verify_document_fact(
        &self,
        frame: &EditorFrameModel,
        document: EditorDocumentId,
        expected: SceneAuthoringRevision,
    ) -> Result<(), EditorActionRejection> {
        let Some(frame_document) = frame.workspace().document(document) else {
            return Err(EditorActionRejection::DocumentNotInFrame { document });
        };
        if frame_document.revision != expected {
            return Err(EditorActionRejection::StaleDocumentFact {
                document,
                expected,
                actual: Some(frame_document.revision),
            });
        }
        let actual = self.workspace.document_revision(document);
        if actual != Some(expected) {
            return Err(EditorActionRejection::StaleDocumentFact {
                document,
                expected,
                actual,
            });
        }
        Ok(())
    }

    fn ui_authoring_field_edit_matches(
        &self,
        frame: &EditorFrameModel,
        edit: &EditorAtomicFieldEdit,
        mode: EditorFieldCommitMode,
    ) -> bool {
        #[cfg(feature = "runtime-ui")]
        {
            let policy = match mode {
                EditorFieldCommitMode::Discrete => {
                    nara_ui::__private::UiAuthoringCommitPolicy::Discrete
                }
                EditorFieldCommitMode::Continuous => {
                    nara_ui::__private::UiAuthoringCommitPolicy::Continuous
                }
            };
            self.ui_authoring_frame.as_deref().is_some_and(|authoring| {
                self.workspace.active_document() == Some(edit.target().document())
                    && authoring.editor_frame() == frame.revision()
                    && authoring.admits_field_edit(edit, policy)
            })
        }
        #[cfg(not(feature = "runtime-ui"))]
        {
            let _ = (frame, edit, mode);
            false
        }
    }

    fn ui_authoring_structure_matches(
        &self,
        frame: &EditorFrameModel,
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        command: &nara_tooling::PrefabCompositionCommand,
    ) -> bool {
        #[cfg(feature = "runtime-ui")]
        {
            self.ui_authoring_frame.as_deref().is_some_and(|authoring| {
                self.workspace.active_document() == Some(document)
                    && authoring.editor_frame() == frame.revision()
                    && authoring.document() == document
                    && authoring.revision() == base_revision
                    && authoring.admits_structure_command(command)
            })
        }
        #[cfg(not(feature = "runtime-ui"))]
        {
            let _ = (frame, document, base_revision, command);
            false
        }
    }

    fn cancel_editor_gesture(
        &mut self,
        reason: nara_tooling::EditorGestureCancelReason,
    ) -> EditorGestureOutcome {
        match self.gestures.cancel_active(reason) {
            Some(outcome) => {
                self.last_gesture_cancellation = Some(reason);
                outcome
            }
            None => EditorGestureOutcome::Rejected(EditorGestureRejection::NoActiveGesture),
        }
    }

    fn verify_runtime_fact(
        &self,
        frame: &EditorFrameModel,
        document: EditorDocumentId,
        generation: u64,
    ) -> Result<(), EditorActionRejection> {
        if frame.play().generation() != Some(generation) {
            return Err(EditorActionRejection::RuntimeGenerationChanged {
                expected: generation,
                actual: frame.play().generation(),
            });
        }
        let actual_document = self
            .trusted_compiled()
            .and_then(|lane| match &lane.runtime_owner {
                EditorRuntimeOwnerState::Active { document, .. } => Some(*document),
                _ => None,
            });
        if actual_document != Some(document) {
            return Err(EditorActionRejection::RuntimeDocumentChanged {
                expected: document,
                actual: actual_document,
            });
        }
        let actual_generation = self
            .trusted_compiled()
            .and_then(|lane| lane.runtime_host.running_runtime_generation());
        if actual_generation != Some(generation) {
            return Err(EditorActionRejection::RuntimeGenerationChanged {
                expected: generation,
                actual: actual_generation,
            });
        }
        Ok(())
    }
}

fn editor_action_rejection_diagnostic(rejection: EditorActionRejection) -> Diagnostic {
    let (code, summary) = match rejection {
        EditorActionRejection::NoPublishedFrame => (
            "editor.action.no-published-frame",
            "Editor action arrived before a frame was published",
        ),
        EditorActionRejection::StaleFrame { .. } => (
            "editor.action.stale-frame",
            "Editor action targets an obsolete frame",
        ),
        EditorActionRejection::FrameAlreadyDispatched { .. } => (
            "editor.action.frame-already-dispatched",
            "Editor frame already accepted an action batch",
        ),
        EditorActionRejection::DocumentNotInFrame { .. } => (
            "editor.action.document-not-in-frame",
            "Editor action targets a document absent from its frame",
        ),
        EditorActionRejection::StaleDocumentFact { .. } => (
            "editor.action.stale-document",
            "Editor action targets an obsolete document revision",
        ),
        EditorActionRejection::ActiveDocumentChanged { .. } => (
            "editor.action.active-document-changed",
            "Editor action targets a document that is no longer active",
        ),
        EditorActionRejection::MutationAlreadySubmitted { .. } => (
            "editor.action.duplicate-mutation",
            "Editor frame already submitted a mutation for this document",
        ),
        EditorActionRejection::FieldContractMismatch { .. } => (
            "editor.action.field-contract-mismatch",
            "Editor field edit does not match its published descriptor",
        ),
        EditorActionRejection::StructureContractMismatch { .. } => (
            "editor.action.structure-contract-mismatch",
            "Editor structure edit does not match its published UI authoring frame",
        ),
        EditorActionRejection::RuntimeGenerationChanged { .. } => (
            "editor.action.stale-runtime-generation",
            "Editor action targets an obsolete runtime generation",
        ),
        EditorActionRejection::RuntimeDocumentChanged { .. } => (
            "editor.action.stale-runtime-document",
            "Editor action targets a runtime owned by another document",
        ),
        EditorActionRejection::InvalidReopenTarget => (
            "editor.action.invalid-reopen-target",
            "Editor action carries an obsolete Reopen target",
        ),
    };
    let mut diagnostic = Diagnostic::error(
        DiagnosticCode::new(code).expect("Editor action diagnostic codes are engine-owned"),
        SafeSummary::new(summary).expect("Editor action diagnostic summaries are safe literals"),
    );
    match rejection {
        EditorActionRejection::DocumentNotInFrame { document }
        | EditorActionRejection::MutationAlreadySubmitted { document }
        | EditorActionRejection::FieldContractMismatch { document }
        | EditorActionRejection::StructureContractMismatch { document } => {
            diagnostic = with_public_u64(diagnostic, "document", document.raw());
        }
        EditorActionRejection::StaleDocumentFact {
            document,
            expected,
            actual,
        } => {
            diagnostic = with_public_u64(diagnostic, "document", document.raw());
            diagnostic = with_public_u64(
                diagnostic,
                "expected_revision_generation",
                expected.generation(),
            );
            if let Some(actual) = actual {
                diagnostic = with_public_u64(
                    diagnostic,
                    "actual_revision_generation",
                    actual.generation(),
                );
            }
        }
        EditorActionRejection::ActiveDocumentChanged { expected, actual } => {
            diagnostic = with_public_u64(diagnostic, "expected_document", expected.raw());
            if let Some(actual) = actual {
                diagnostic = with_public_u64(diagnostic, "actual_document", actual.raw());
            }
        }
        EditorActionRejection::RuntimeGenerationChanged { expected, actual } => {
            diagnostic = with_public_u64(diagnostic, "expected_runtime_generation", expected);
            if let Some(actual) = actual {
                diagnostic = with_public_u64(diagnostic, "actual_runtime_generation", actual);
            }
        }
        EditorActionRejection::RuntimeDocumentChanged { expected, actual } => {
            diagnostic = with_public_u64(diagnostic, "expected_document", expected.raw());
            if let Some(actual) = actual {
                diagnostic = with_public_u64(diagnostic, "actual_document", actual.raw());
            }
        }
        EditorActionRejection::NoPublishedFrame
        | EditorActionRejection::StaleFrame { .. }
        | EditorActionRejection::FrameAlreadyDispatched { .. }
        | EditorActionRejection::InvalidReopenTarget => {}
    }
    diagnostic
}

fn editor_field_validation_diagnostic(failure: &EditorFieldValidationFailure) -> Diagnostic {
    let mut diagnostic = Diagnostic::error(
        DiagnosticCode::new("editor.field.invalid-value")
            .expect("Editor field diagnostic codes are engine-owned"),
        SafeSummary::new("Editor field value does not match its declared type")
            .expect("Editor field diagnostic summaries are safe literals"),
    );
    diagnostic = with_public_u64(diagnostic, "document", failure.target().document().raw());
    with_public_u64(
        diagnostic,
        "revision_generation",
        failure.target().base_revision().generation(),
    )
}

fn with_public_u64(diagnostic: Diagnostic, key: &'static str, value: u64) -> Diagnostic {
    diagnostic
        .try_with_field(DiagnosticField::public_u64(
            DiagnosticFieldKey::new(key).expect("Editor action diagnostic keys are engine-owned"),
            value,
        ))
        .expect("Editor action diagnostics use unique fields within the hard limit")
}
