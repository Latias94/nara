use std::sync::Arc;

use nara_diagnostic::{
    Diagnostic, DiagnosticCode, DiagnosticField, DiagnosticFieldKey, SafeSummary,
};
use nara_scene::SceneAuthoringRevision;
use nara_tooling::{
    EditorAction, EditorActionAcknowledgement, EditorActionBatch, EditorActionBatchReport,
    EditorActionOutcome, EditorActionRejection, EditorAtomicFieldEdit, EditorDocumentId,
    EditorDocumentMutation, EditorFieldFeedback, EditorFieldFeedbackReason, EditorFieldTarget,
    EditorFieldValidationFailure, EditorFrameModel, EditorGestureAction, EditorGestureCancelReason,
    EditorGestureOutcome, EditorGestureRejection, EditorImageImportAction,
    EditorImageImportActionOutcome, EditorImageImportActionRejection, EditorNavigationAction,
    EditorPersistenceAction, EditorPersistenceCommand, EditorPlayAction, EditorPlayCommand,
    EditorReopenTarget, EditorSourceToolAction, EditorSourceToolActionOutcome,
    EditorWorkspaceCommand, EditorWorkspaceIntent, EditorWorkspaceIntentAction,
};

use crate::project_host::authoring::ProjectSourceIndexSnapshot;

use super::{EditorProjectSession, EditorRuntimeOwnerState};

impl EditorProjectSession {
    /// Applies ordered actions against their owning document or displayed-frame authority.
    ///
    /// Ordinary document commands use their explicit live base revision. Frame-sensitive actions
    /// retain exact frame admission; neither navigation nor acknowledgements consume a batch slot.
    pub fn dispatch_editor_actions(
        &mut self,
        batch: EditorActionBatch,
    ) -> Result<EditorActionBatchReport, EditorActionRejection> {
        let batch_field_target = batch.first_field_target();
        let received = batch.frame_revision();
        let frame = match self.published_frame.as_ref() {
            None => Err(EditorActionRejection::NoPublishedFrame),
            Some(published) => {
                let expected = published.model.revision();
                if received != expected && batch.requires_current_frame() {
                    Err(EditorActionRejection::StaleFrame { expected, received })
                } else {
                    Ok((
                        Arc::clone(&published.model),
                        published.source_index.as_ref().map(Arc::clone),
                        published.image_import.clone(),
                    ))
                }
            }
        };
        let (frame, source_index, image_import) = match frame {
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
        let outcomes = batch.into_actions().into_iter().map(|action| {
            let field_target = action.field_target();
            let outcome = self.dispatch_editor_action(
                &frame,
                source_index.as_ref(),
                image_import.as_ref(),
                action,
            );
            if let EditorActionOutcome::Rejected(rejection) = &outcome {
                self.record_editor_action_rejection(*rejection);
            }
            self.update_editor_field_feedback(field_target, &outcome);
            outcome
        });
        Ok(EditorActionBatchReport::new(received, outcomes))
    }

    fn record_editor_action_rejection(&mut self, rejection: EditorActionRejection) {
        self.diagnostics
            .push(editor_action_rejection_diagnostic(rejection));
    }

    fn dispatch_editor_action(
        &mut self,
        frame: &EditorFrameModel,
        source_index: Option<&Arc<ProjectSourceIndexSnapshot>>,
        image_import: Option<&super::image_import::EditorImageImportFrame>,
        action: EditorAction,
    ) -> EditorActionOutcome {
        match action {
            EditorAction::Navigate(action) => self.dispatch_navigation(frame, action),
            EditorAction::QueryDocumentEntities(query) => {
                EditorActionOutcome::DocumentEntities(self.workspace.query_document_entities(query))
            }
            EditorAction::QueryDocumentAuthoring(query) => EditorActionOutcome::DocumentAuthoring(
                self.workspace.query_document_authoring(query),
            ),
            EditorAction::ReadDocumentProperty(request) => EditorActionOutcome::DocumentProperty(
                self.workspace.read_document_property(request),
            ),
            EditorAction::CommitAuthoringEdit(prepared) => self.commit_authoring_edit(prepared),
            EditorAction::CommitAuthoringConstruction(prepared) => {
                match self.commit_authoring_construction(prepared) {
                    Ok(committed) => EditorActionOutcome::AuthoringConstruction(committed),
                    Err(outcome) => outcome,
                }
            }
            EditorAction::PrefabConversion(action) => {
                EditorActionOutcome::PrefabConversion(self.request_prefab_conversion(action))
            }
            EditorAction::SourceTool(action) => self.dispatch_source_tool_action(action),
            EditorAction::Mutate(action) => self.dispatch_document_mutation(action),
            EditorAction::Extension(action) => {
                if let Err(rejection) =
                    self.verify_document_revision(action.document(), action.base_revision())
                {
                    return EditorActionOutcome::Rejected(rejection);
                }
                if !self.accepts_new_work() || self.pending_workspace_intent.is_some() {
                    return EditorActionOutcome::Workspace(
                        super::editor_workspace_command_rejected(
                            self.workspace.active_document(),
                            "project.editor.workspace-intent-pending",
                        ),
                    );
                }
                let report = self.workspace.apply_extension_action(action);
                let _ = self.diagnostics.extend(report.diagnostics.clone());
                EditorActionOutcome::Workspace(report)
            }
            EditorAction::Persistence(action) => self.dispatch_persistence(action),
            EditorAction::ProjectBrowser(action) => EditorActionOutcome::ProjectBrowser(
                self.dispatch_project_browser_action(frame, source_index, action),
            ),
            EditorAction::ImageImport(action) => {
                self.dispatch_image_import(frame, image_import, action)
            }
            EditorAction::SourceAssetImport(action) => EditorActionOutcome::SourceAssetImport(
                self.dispatch_source_asset_import(source_index, action),
            ),
            EditorAction::ProjectContent(action) => {
                EditorActionOutcome::ProjectContent(self.request_project_content_refresh(action))
            }
            EditorAction::WorkspaceIntent(action) => self.dispatch_workspace_intent(frame, action),
            EditorAction::Play(action) => self.dispatch_play(frame, action),
            EditorAction::ReportFieldValidation(failure) => self.dispatch_field_validation(failure),
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

    fn dispatch_image_import(
        &mut self,
        frame: &EditorFrameModel,
        image_frame: Option<&super::image_import::EditorImageImportFrame>,
        action: EditorImageImportAction,
    ) -> EditorActionOutcome {
        if self.source_asset_import.has_pending_work() {
            return EditorActionOutcome::ImageImport(EditorImageImportActionOutcome::Rejected(
                EditorImageImportActionRejection::Busy,
            ));
        }
        let EditorImageImportAction::Submit {
            relative_path,
            expected_detail_revision,
            settings,
        } = action;
        let Some(view) = frame.image_import() else {
            return EditorActionOutcome::ImageImport(EditorImageImportActionOutcome::Rejected(
                EditorImageImportActionRejection::Unavailable,
            ));
        };
        if view.relative_path() != relative_path.as_ref()
            || view.detail_revision() != expected_detail_revision
        {
            return EditorActionOutcome::ImageImport(EditorImageImportActionOutcome::Rejected(
                EditorImageImportActionRejection::Stale,
            ));
        }
        if !view.can_submit() {
            let rejection = match view.attempt() {
                nara_tooling::EditorImageImportAttemptView::Pending { .. } => {
                    EditorImageImportActionRejection::Busy
                }
                nara_tooling::EditorImageImportAttemptView::ReconcileRequired { .. } => {
                    EditorImageImportActionRejection::ReconcileRequired
                }
                nara_tooling::EditorImageImportAttemptView::Idle
                | nara_tooling::EditorImageImportAttemptView::Succeeded { .. }
                | nara_tooling::EditorImageImportAttemptView::Failed { .. }
                | nara_tooling::EditorImageImportAttemptView::Stale { .. }
                | nara_tooling::EditorImageImportAttemptView::Cancelled { .. } => {
                    EditorImageImportActionRejection::MetadataBlocked
                }
            };
            return EditorActionOutcome::ImageImport(EditorImageImportActionOutcome::Rejected(
                rejection,
            ));
        }
        let Some(image_frame) = image_frame.cloned() else {
            return EditorActionOutcome::ImageImport(EditorImageImportActionOutcome::Rejected(
                EditorImageImportActionRejection::Unavailable,
            ));
        };
        if image_frame.relative_path() != relative_path.as_ref()
            || image_frame.detail_revision() != expected_detail_revision.get()
        {
            return EditorActionOutcome::ImageImport(EditorImageImportActionOutcome::Rejected(
                EditorImageImportActionRejection::Stale,
            ));
        }
        let result = self.submit_editor_image_import(
            image_frame,
            &relative_path,
            expected_detail_revision.get(),
            super::image_import::domain_settings(settings),
        );
        EditorActionOutcome::ImageImport(super::image_import::project_request_outcome(result))
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
        failure: EditorFieldValidationFailure,
    ) -> EditorActionOutcome {
        if let Err(rejection) = self.verify_document_revision(
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
            EditorActionOutcome::DocumentEntities(_)
            | EditorActionOutcome::DocumentAuthoring(_)
            | EditorActionOutcome::DocumentProperty(_)
            | EditorActionOutcome::AuthoringConstruction(_)
            | EditorActionOutcome::PrefabConversion(_)
            | EditorActionOutcome::SourceTool(_)
            | EditorActionOutcome::Persistence(_)
            | EditorActionOutcome::ProjectBrowser(_)
            | EditorActionOutcome::ImageImport(_)
            | EditorActionOutcome::SourceAssetImport(_)
            | EditorActionOutcome::ProjectContent(_)
            | EditorActionOutcome::WorkspaceIntent(_)
            | EditorActionOutcome::Play(_)
            | EditorActionOutcome::Gesture(_)
            | EditorActionOutcome::RuntimeEdit(_)
            | EditorActionOutcome::ApplyChanges(_)
            | EditorActionOutcome::Acknowledged(_) => {}
        }
    }

    fn dispatch_source_tool_action(
        &mut self,
        action: EditorSourceToolAction,
    ) -> EditorActionOutcome {
        match action {
            EditorSourceToolAction::Request {
                activation,
                basis,
                intent,
                parameters,
            } => EditorActionOutcome::SourceTool(EditorSourceToolActionOutcome::Requested(
                self.request_source_tool(&activation, &basis, intent, parameters),
            )),
            EditorSourceToolAction::IntegerProperty {
                activation,
                target,
                value,
            } => EditorActionOutcome::SourceTool(EditorSourceToolActionOutcome::Requested(
                self.request_source_tool_integer_property(&activation, target, value),
            )),
            EditorSourceToolAction::Accept { request } => EditorActionOutcome::SourceTool(
                EditorSourceToolActionOutcome::Accepted(self.accept_source_tool_review(&request)),
            ),
            EditorSourceToolAction::Disable { activation } => EditorActionOutcome::SourceTool(
                EditorSourceToolActionOutcome::Disabled(self.disable_source_tool(&activation)),
            ),
            EditorSourceToolAction::CancelAttempt { attempt } => EditorActionOutcome::SourceTool(
                EditorSourceToolActionOutcome::Cancelled(self.cancel_source_tool_attempt(&attempt)),
            ),
            EditorSourceToolAction::DisableInstalled { basis } => {
                EditorActionOutcome::SourceTool(EditorSourceToolActionOutcome::Selection(
                    self.disable_installed_source_tool(&basis),
                ))
            }
            EditorSourceToolAction::ReconcileSelection { basis } => {
                EditorActionOutcome::SourceTool(EditorSourceToolActionOutcome::Selection(
                    self.reconcile_source_tool_selection(&basis),
                ))
            }
        }
    }

    fn dispatch_document_mutation(
        &mut self,
        action: EditorDocumentMutation,
    ) -> EditorActionOutcome {
        let document = action.document();
        let base_revision = action.base_revision();
        if let Err(rejection) = self.verify_document_revision(document, base_revision) {
            return EditorActionOutcome::Rejected(rejection);
        }
        if !self.accepts_new_work() {
            return EditorActionOutcome::Workspace(super::editor_workspace_command_rejected(
                self.workspace.active_document(),
                "project.editor.workspace-intent-pending",
            ));
        }
        if let EditorDocumentMutation::ApplyPrefabFieldEdit { edit } = &action
            && !self.ui_authoring_document_field_edit_matches(edit)
        {
            return EditorActionOutcome::Rejected(EditorActionRejection::FieldContractMismatch {
                document,
            });
        }
        if let EditorDocumentMutation::ComposePrefab { command, .. } = &action
            && (self.prefab_uses_ui_authoring_domain(document)
                || prefab_command_uses_ui_domain(command))
            && !self.ui_authoring_structure_matches(document, command)
        {
            return EditorActionOutcome::Rejected(
                EditorActionRejection::StructureContractMismatch { document },
            );
        }
        if let EditorDocumentMutation::ApplyPrefabPatch { patch, .. } = &action
            && let Err(rejection) = self.verify_prepared_patch_contract(document, patch)
        {
            return EditorActionOutcome::Rejected(rejection);
        }
        if let EditorDocumentMutation::ApplyInspectorCommand {
            command: nara_tooling::SceneInspectorCommand::MoveEntity { .. },
            ..
        } = &action
            && self.prefab_uses_ui_authoring_domain(document)
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
            EditorDocumentMutation::ApplyAuthoringFieldEdit {
                document,
                base_revision,
                edit,
            } => EditorWorkspaceCommand::ApplyAuthoringFieldEdit {
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
        if !self.accepts_new_work() && !matches!(&action, EditorGestureAction::Cancel { .. }) {
            return EditorActionOutcome::Workspace(super::editor_workspace_command_rejected(
                self.workspace.active_document(),
                "project.editor.workspace-intent-pending",
            ));
        }
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
                    || !self.ui_authoring_field_edit_matches(frame, &baseline)
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
                if !self.ui_authoring_field_edit_matches(frame, &edit) {
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
                if !self.ui_authoring_field_edit_matches(frame, &edit) {
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

    fn dispatch_persistence(&mut self, action: EditorPersistenceAction) -> EditorActionOutcome {
        match action {
            EditorPersistenceAction::Save {
                document,
                base_revision,
            } => {
                if let Err(rejection) = self.verify_document_revision(document, base_revision) {
                    return EditorActionOutcome::Rejected(rejection);
                }
                EditorActionOutcome::Persistence(self.request_persistence(
                    EditorPersistenceCommand::Save {
                        document: Some(document),
                    },
                ))
            }
            EditorPersistenceAction::Reopen { target } => {
                let document = match target {
                    EditorReopenTarget::OpenDocument {
                        document,
                        base_revision,
                    } => {
                        if let Err(rejection) =
                            self.verify_document_revision(document, base_revision)
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
                if matches!(
                    outcome,
                    nara_tooling::EditorPersistenceRequestResult::Accepted { .. }
                ) {
                    let _ = self.cancel_editor_gesture(EditorGestureCancelReason::Reopen);
                }
                EditorActionOutcome::Persistence(outcome)
            }
            EditorPersistenceAction::AcknowledgeResult { attempt } => {
                EditorActionOutcome::Persistence(
                    self.request_persistence(EditorPersistenceCommand::AcknowledgeResult {
                        attempt,
                    }),
                )
            }
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
            EditorWorkspaceIntentAction::Resolve { prompt, decision } => {
                EditorActionOutcome::WorkspaceIntent(
                    self.resolve_workspace_intent(prompt, decision),
                )
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
            EditorPlayAction::RunLastGood => self.request_play(EditorPlayCommand::RunLastGood),
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
        self.verify_document_revision(document, expected)
    }

    fn verify_document_revision(
        &self,
        document: EditorDocumentId,
        expected: SceneAuthoringRevision,
    ) -> Result<(), EditorActionRejection> {
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
    ) -> bool {
        #[cfg(feature = "runtime-ui")]
        {
            self.ui_authoring_frame.as_deref().is_some_and(|authoring| {
                self.workspace.active_document() == Some(edit.target().document())
                    && authoring.editor_frame() == frame.revision()
                    && authoring.admits_field_edit(
                        edit,
                        nara_ui::__private::UiAuthoringCommitPolicy::Continuous,
                    )
            })
        }
        #[cfg(not(feature = "runtime-ui"))]
        {
            let _ = (frame, edit);
            false
        }
    }

    pub(super) fn prefab_uses_ui_authoring_domain(&self, document: EditorDocumentId) -> bool {
        self.workspace.prefab(document).is_some_and(|slot| {
            slot.session().document().entities.iter().any(|entity| {
                entity.components.keys().any(component_uses_ui_domain)
                    || entity
                        .prefab
                        .as_ref()
                        .is_some_and(prefab_instance_uses_ui_domain)
            })
        })
    }

    fn ui_authoring_structure_matches(
        &self,
        document: EditorDocumentId,
        command: &nara_tooling::PrefabCompositionCommand,
    ) -> bool {
        #[cfg(feature = "runtime-ui")]
        {
            let Some(slot) = self.workspace.prefab(document) else {
                return false;
            };
            let Ok(projection) =
                nara_ui::__private::project_prefab_authoring(slot.session().document())
            else {
                return false;
            };
            let choices = self.authoring_images.receipt();
            super::ui_authoring::projection_admits_structure_command(
                &projection,
                command,
                Some(choices.share_choices().as_ref()),
            )
        }
        #[cfg(not(feature = "runtime-ui"))]
        {
            let _ = (document, command);
            false
        }
    }

    fn ui_authoring_document_field_edit_matches(&self, edit: &EditorAtomicFieldEdit) -> bool {
        #[cfg(feature = "runtime-ui")]
        {
            let Some(slot) = self.workspace.prefab(edit.target().document()) else {
                return false;
            };
            let Ok(projection) =
                nara_ui::__private::project_prefab_authoring(slot.session().document())
            else {
                return false;
            };
            super::ui_authoring::projection_admits_field_edit(
                &projection,
                edit,
                nara_ui::__private::UiAuthoringCommitPolicy::Discrete,
            )
        }
        #[cfg(not(feature = "runtime-ui"))]
        {
            let _ = edit;
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
                EditorRuntimeOwnerState::Active { source, .. } => Some(source.document),
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

fn component_uses_ui_domain(component: &nara_reflect::ComponentTypeId) -> bool {
    // Persistent domain identity remains meaningful when its executable module is absent.
    matches!(
        component.as_str(),
        "nara.ui.UiRoot"
            | "nara.ui.UiNode"
            | "nara.ui.UiVisual"
            | "nara.ui.UiText"
            | "nara.ui.UiButton"
            | "nara.ui.UiButtonVisuals"
            | "nara.ui.UiSemantics"
            | "nara.ui.UiTheme"
    )
}

fn prefab_command_uses_ui_domain(command: &nara_tooling::PrefabCompositionCommand) -> bool {
    use nara_tooling::PrefabCompositionCommand;
    match command {
        PrefabCompositionCommand::CreateEntity { draft, .. } => {
            draft.components().keys().any(component_uses_ui_domain)
        }
        PrefabCompositionCommand::AddComponent { component, .. }
        | PrefabCompositionCommand::RemoveComponent { component, .. } => {
            component_uses_ui_domain(component)
        }
        PrefabCompositionCommand::MoveEntity { .. }
        | PrefabCompositionCommand::RemoveEntity { .. } => false,
    }
}

pub(super) fn prefab_patch_uses_ui_domain(patch: &nara_scene::ScenePatchDocument) -> bool {
    use nara_scene::ScenePatchOperation;
    let mut pending = patch.operations.iter().collect::<Vec<_>>();
    while let Some(operation) = pending.pop() {
        match operation {
            ScenePatchOperation::SetPrefabInstance {
                expected, value, ..
            } => {
                for prefab in expected.iter().chain(value.iter()) {
                    if prefab_instance_uses_ui_domain(prefab) {
                        return true;
                    }
                }
            }
            ScenePatchOperation::AddEntity { entity, .. } => {
                if entity.components.keys().any(component_uses_ui_domain) {
                    return true;
                }
                if let Some(prefab) = &entity.prefab {
                    if prefab_instance_uses_ui_domain(prefab) {
                        return true;
                    }
                }
            }
            ScenePatchOperation::RestoreEntity {
                entity,
                descendants,
                ..
            } => {
                for entity in std::iter::once(entity).chain(descendants.iter()) {
                    if entity.components.keys().any(component_uses_ui_domain) {
                        return true;
                    }
                    if let Some(prefab) = &entity.prefab {
                        if prefab_instance_uses_ui_domain(prefab) {
                            return true;
                        }
                    }
                }
            }
            ScenePatchOperation::AddComponent { component, .. }
            | ScenePatchOperation::RemoveComponent { component, .. }
            | ScenePatchOperation::ReplaceComponent { component, .. }
            | ScenePatchOperation::SetField { component, .. }
            | ScenePatchOperation::RemoveField { component, .. }
            | ScenePatchOperation::SetAssetRefField { component, .. } => {
                if component_uses_ui_domain(component) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn prefab_instance_uses_ui_domain(instance: &nara_scene::PrefabInstance) -> bool {
    use nara_scene::PrefabComponentIntent;
    let mut pending = instance.overrides.iter().collect::<Vec<_>>();
    while let Some(overlay) = pending.pop() {
        for addition in &overlay.additions {
            if addition.components.keys().any(component_uses_ui_domain) {
                return true;
            }
            if let Some(instance) = &addition.prefab {
                pending.extend(instance.overrides.iter());
            }
        }
        for claim in &overlay.instances {
            if let Some(instance) = &claim.value {
                pending.extend(instance.overrides.iter());
            }
        }
        if overlay.components.iter().any(|intent| {
            let component = match intent {
                PrefabComponentIntent::Set { component, .. }
                | PrefabComponentIntent::Suppress { component, .. }
                | PrefabComponentIntent::Properties { component, .. } => component,
            };
            component_uses_ui_domain(component)
        }) {
            return true;
        }
    }
    false
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

#[cfg(test)]
mod tests {
    use super::prefab_patch_uses_ui_domain;
    use nara_asset::AssetRef;
    use nara_reflect::{ComponentFieldId, ComponentSchemaVersion, ComponentTypeId, ComponentValue};
    use nara_scene::{
        PrefabComponentIntent, PrefabEntityTarget, PrefabInstance, PrefabInstanceClaim,
        PrefabOverlay, PrefabPropertyClaim, PrefabPropertyIntent, SceneEntityId,
        ScenePatchDocument, ScenePatchOperation, SourceDocumentKey, SourceDocumentKind,
    };

    #[test]
    fn ui_domain_guard_checks_nested_expected_and_value_snapshots() {
        let id = SceneEntityId::new("nested").unwrap();
        let instance = |overrides| PrefabInstance {
            source: AssetRef::path("base.prefab.json").unwrap(),
            overrides,
        };
        let overlay = || {
            PrefabOverlay::new(
                SourceDocumentKey::named(
                    "92387b8a-a667-47e5-bda4-6c0c77cc84df".parse().unwrap(),
                    "ui.guard",
                    SourceDocumentKind::Prefab,
                )
                .unwrap(),
            )
        };
        let mut field = overlay();
        field.components.push(PrefabComponentIntent::Properties {
            target: PrefabEntityTarget::source(id.clone()),
            component: ComponentTypeId::new("nara.ui.UiText"),
            version: ComponentSchemaVersion::ONE,
            claims: vec![PrefabPropertyClaim {
                path: nara_reflect::ComponentPropertyPath::new(ComponentFieldId::new("text")),
                intent: PrefabPropertyIntent::Set(ComponentValue::String(
                    "authored text".to_owned(),
                )),
                bindings: Vec::new(),
            }],
        });
        for outer_expected in [false, true] {
            for added_anchor in [false, true] {
                let ui = instance(Some(field.clone()));
                let mut inner = overlay();
                if added_anchor {
                    let mut addition = nara_scene::PrefabAddition::new(id.clone());
                    addition.prefab = Some(ui);
                    inner.additions.push(addition);
                } else {
                    inner.instances.push(PrefabInstanceClaim {
                        target: PrefabEntityTarget::source(id.clone()),
                        value: Some(ui),
                    });
                }
                let nested = Some(instance(Some(inner)));
                let outer = ScenePatchDocument::new([ScenePatchOperation::SetPrefabInstance {
                    entity: id.clone(),
                    expected: if outer_expected { nested.clone() } else { None },
                    value: if outer_expected { None } else { nested },
                }]);
                assert!(prefab_patch_uses_ui_domain(&outer));
            }
        }
        assert!(!prefab_patch_uses_ui_domain(&ScenePatchDocument::new([
            ScenePatchOperation::SetPrefabInstance {
                entity: id,
                expected: Some(instance(None)),
                value: None,
            },
        ])));
    }
}
