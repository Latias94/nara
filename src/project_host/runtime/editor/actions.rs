use std::{collections::BTreeSet, sync::Arc};

use nara_diagnostic::{
    Diagnostic, DiagnosticCode, DiagnosticField, DiagnosticFieldKey, SafeSummary,
};
use nara_scene::SceneAuthoringRevision;
use nara_tooling::{
    EditorAction, EditorActionAcknowledgement, EditorActionBatch, EditorActionBatchReport,
    EditorActionOutcome, EditorActionRejection, EditorDocumentId, EditorDocumentMutation,
    EditorFieldFeedback, EditorFieldFeedbackReason, EditorFieldTarget,
    EditorFieldValidationFailure, EditorFrameModel, EditorNavigationAction,
    EditorPersistenceAction, EditorPersistenceCommand, EditorPlayAction, EditorPlayCommand,
    EditorReopenTarget, EditorWorkspaceCommand, EditorWorkspaceIntent, EditorWorkspaceIntentAction,
};

use super::{EditorProjectSession, EditorRuntimeOwnerState};

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
                    Ok(Arc::clone(&published.model))
                }
            }
        };
        let frame = match frame {
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
                self.dispatch_editor_action(&frame, action)
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
        action: EditorAction,
    ) -> EditorActionOutcome {
        match action {
            EditorAction::Navigate(action) => self.dispatch_navigation(frame, action),
            EditorAction::Mutate(action) => self.dispatch_document_mutation(frame, action),
            EditorAction::Persistence(action) => self.dispatch_persistence(frame, action),
            EditorAction::WorkspaceIntent(action) => self.dispatch_workspace_intent(frame, action),
            EditorAction::Play(action) => self.dispatch_play(frame, action),
            EditorAction::ReportFieldValidation(failure) => {
                self.dispatch_field_validation(frame, failure)
            }
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
            return EditorActionOutcome::Workspace(report);
        }
        let document = action.document();
        if frame.workspace().document(document).is_none() {
            return EditorActionOutcome::Rejected(EditorActionRejection::DocumentNotInFrame {
                document,
            });
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
        EditorActionOutcome::Workspace(self.apply_workspace_command(command))
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
            | EditorActionOutcome::WorkspaceIntent(_)
            | EditorActionOutcome::Play(_)
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
            EditorDocumentMutation::ComposeScene {
                document,
                base_revision,
                command,
            } => EditorWorkspaceCommand::ComposeScene {
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
        let _ = self.diagnostics.extend(report.diagnostics.clone());
        EditorActionOutcome::Workspace(report)
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
                    EditorReopenTarget::ClosedSource { document } => {
                        if self.workspace.document_kind(document).is_some() {
                            return EditorActionOutcome::Rejected(
                                EditorActionRejection::InvalidReopenTarget,
                            );
                        }
                        document
                    }
                };
                EditorActionOutcome::Persistence(self.request_persistence(
                    EditorPersistenceCommand::Reopen {
                        document: Some(document),
                    },
                ))
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
        let actual_document = match self.runtime_owner {
            EditorRuntimeOwnerState::Active { document, .. } => Some(document),
            _ => None,
        };
        if actual_document != Some(document) {
            return Err(EditorActionRejection::RuntimeDocumentChanged {
                expected: document,
                actual: actual_document,
            });
        }
        let actual_generation = self.runtime_host.running_runtime_generation();
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
        EditorActionRejection::MutationAlreadySubmitted { .. } => (
            "editor.action.duplicate-mutation",
            "Editor frame already submitted a mutation for this document",
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
        | EditorActionRejection::MutationAlreadySubmitted { document } => {
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
